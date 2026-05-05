# Recording Waveform Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the elapsed-time-only display during active recording with a live audio waveform, shared across both mic recording (`RecordCard`) and system-audio recording (`RecordCallCard`).

**Architecture:** A presentational `<RecordingWaveform>` canvas is fed by two source-specific hooks: `useMicAmplitude` (Web Audio `AnalyserNode` over the existing `MediaStream`) for mic, and `useTauriAmplitude` (subscribed to a new `call-capture-amplitude` Tauri event) for system audio. The Rust capture loops compute RMS on each incoming audio frame and push it into a per-session bounded channel; a spawned task drains at ~30 fps and emits the event. Both record cards adopt a "waveform-hero" active layout where the waveform is the focal point and the controls have explicit weight.

**Tech Stack:** React + TypeScript (Vite), Tauri 2 (Rust), Web Audio API, `tokio`, `cpal`, ScreenCaptureKit, Vitest, Tailwind tokens.

**Spec:** `docs/superpowers/specs/2026-05-05-recording-waveform-design.md`

---

## File Structure

**New files:**
- `src/components/RecordingWaveform.tsx` — canvas renderer
- `src/components/RecordingWaveform.test.tsx` — pure-function tests (rms, downsample)
- `src/lib/audioAmplitude.ts` — pure helpers: `computeRms`, `RingBuffer`
- `src/lib/audioAmplitude.test.ts` — tests for the helpers
- `src/lib/useMicAmplitude.ts` — Web Audio hook
- `src/lib/useTauriAmplitude.ts` — Tauri event hook
- `src/lib/useTauriAmplitude.test.tsx` — hook test (Tauri event mocks already in `test-setup.ts`)
- `src-tauri/src/capture/amplitude.rs` — `compute_rms` + `AmplitudeEmitter`

**Modified files:**
- `src/components/RecordCard.tsx` — adds `stream` state, replaces active layout
- `src/components/RecordCallCard.tsx` — adds `sessionId` state, replaces active layout
- `src-tauri/src/capture/mic_audio.rs` — accepts emitter, pushes RMS in non-paused path
- `src-tauri/src/capture/system_audio.rs` — same
- `src-tauri/src/capture/mod.rs` — `CallCaptureSession` holds emitter; `begin_session` takes `AppHandle`, builds emitter
- `src-tauri/src/commands.rs` — `start_call_capture` passes `app.clone()` to `begin_session`

**Sequencing:** frontend pure helpers → renderer → mic hook → mic-card integration (mic waveform working end-to-end) → Rust emitter foundation → Rust integration into both audio modules → wire through `begin_session` and `start_call_capture` → Tauri hook → call-card integration.

---

## Task 1: Pure helpers — `computeRms` and `RingBuffer`

**Files:**
- Create: `src/lib/audioAmplitude.ts`
- Test: `src/lib/audioAmplitude.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/audioAmplitude.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { computeRms, RingBuffer } from "./audioAmplitude";

describe("computeRms", () => {
  it("returns 0 for silence", () => {
    const buf = new Float32Array(1024);
    expect(computeRms(buf)).toBe(0);
  });

  it("returns ~1/sqrt(2) for a unit sine wave", () => {
    const buf = new Float32Array(1024);
    for (let i = 0; i < buf.length; i++) {
      buf[i] = Math.sin((2 * Math.PI * i) / 16);
    }
    expect(computeRms(buf)).toBeCloseTo(1 / Math.SQRT2, 2);
  });

  it("clamps to [0,1]", () => {
    const buf = new Float32Array(8).fill(2);
    expect(computeRms(buf)).toBe(1);
  });
});

describe("RingBuffer", () => {
  it("starts empty (returns zeros)", () => {
    const r = new RingBuffer(4);
    expect(Array.from(r.snapshot())).toEqual([0, 0, 0, 0]);
  });

  it("pushes values into a fixed-size window, oldest first", () => {
    const r = new RingBuffer(4);
    r.push(0.1);
    r.push(0.2);
    r.push(0.3);
    expect(Array.from(r.snapshot())).toEqual([0, 0.1, 0.2, 0.3]);
  });

  it("wraps when overflowing", () => {
    const r = new RingBuffer(3);
    r.push(0.1);
    r.push(0.2);
    r.push(0.3);
    r.push(0.4);
    r.push(0.5);
    expect(Array.from(r.snapshot())).toEqual([0.3, 0.4, 0.5]);
  });

  it("returns a new Float32Array reference each snapshot (so React sees changes)", () => {
    const r = new RingBuffer(2);
    const a = r.snapshot();
    r.push(0.1);
    const b = r.snapshot();
    expect(a).not.toBe(b);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/audioAmplitude.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement helpers**

Create `src/lib/audioAmplitude.ts`:

```ts
/** RMS over an interleaved or mono float frame, clamped to [0,1]. */
export function computeRms(samples: Float32Array): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < samples.length; i++) {
    sum += samples[i] * samples[i];
  }
  const rms = Math.sqrt(sum / samples.length);
  return Math.min(1, Math.max(0, rms));
}

/** Fixed-size ring of floats. `snapshot()` returns a fresh copy each call so
 * React change detection works on the array reference. */
export class RingBuffer {
  private buf: Float32Array;
  private writeIdx = 0;
  private filled = 0;

  constructor(public readonly capacity: number) {
    this.buf = new Float32Array(capacity);
  }

  push(value: number): void {
    this.buf[this.writeIdx] = value;
    this.writeIdx = (this.writeIdx + 1) % this.capacity;
    if (this.filled < this.capacity) this.filled++;
  }

  /** Oldest → newest. Returns a fresh Float32Array. */
  snapshot(): Float32Array {
    const out = new Float32Array(this.capacity);
    if (this.filled < this.capacity) {
      // No wrap yet: data lives in [0, writeIdx).
      out.set(this.buf.subarray(0, this.writeIdx), this.capacity - this.writeIdx);
      return out;
    }
    // Full: oldest is at writeIdx.
    const tail = this.buf.subarray(this.writeIdx);
    const head = this.buf.subarray(0, this.writeIdx);
    out.set(tail, 0);
    out.set(head, tail.length);
    return out;
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/audioAmplitude.test.ts`
Expected: PASS — 7 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/audioAmplitude.ts src/lib/audioAmplitude.test.ts
git commit -m "feat(audio): add computeRms + RingBuffer helpers"
```

---

## Task 2: `<RecordingWaveform>` component

**Files:**
- Create: `src/components/RecordingWaveform.tsx`
- Create: `src/components/RecordingWaveform.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/components/RecordingWaveform.test.tsx`:

```tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import RecordingWaveform from "./RecordingWaveform";

describe("RecordingWaveform", () => {
  it("renders a canvas element with the requested className", () => {
    const samples = new Float32Array([0.1, 0.2, 0.3]);
    const { container } = render(
      <RecordingWaveform samples={samples} state="recording" className="test-cls" />,
    );
    const canvas = container.querySelector("canvas");
    expect(canvas).not.toBeNull();
    expect(canvas?.className).toContain("test-cls");
  });

  it("re-renders when samples reference changes", () => {
    const a = new Float32Array([0.1]);
    const b = new Float32Array([0.5]);
    const { container, rerender } = render(
      <RecordingWaveform samples={a} state="recording" />,
    );
    const canvas1 = container.querySelector("canvas");
    rerender(<RecordingWaveform samples={b} state="recording" />);
    const canvas2 = container.querySelector("canvas");
    // Same DOM node — React reuses canvas; we just want no crash on re-render.
    expect(canvas1).toBe(canvas2);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/RecordingWaveform.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the component**

Create `src/components/RecordingWaveform.tsx`:

```tsx
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
    const ctx = canvas.getContext("2d");
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

    const color = getComputedStyle(canvas).getPropertyValue(
      state === "recording" ? "--accent" : "--label-tertiary",
    ).trim() || (state === "recording" ? "#0a84ff" : "#999");

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
      className={cn("block w-full h-full", className)}
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/RecordingWaveform.test.tsx`
Expected: PASS — 2 tests.

- [ ] **Step 5: Add downsampleMax tests**

Append to `src/components/RecordingWaveform.test.tsx`:

```tsx
import { downsampleMax } from "./RecordingWaveform";

describe("downsampleMax", () => {
  it("right-aligns when src is shorter than n", () => {
    const out = downsampleMax(new Float32Array([0.5, 0.7]), 4);
    expect(Array.from(out)).toEqual([0, 0, 0.5, 0.7]);
  });

  it("returns max-per-window when src is longer than n", () => {
    const src = new Float32Array([0.1, 0.9, 0.2, 0.3, 0.5, 0.4]);
    const out = downsampleMax(src, 3);
    expect(Array.from(out)).toEqual([0.9, 0.3, 0.5]);
  });

  it("handles empty src", () => {
    const out = downsampleMax(new Float32Array(0), 4);
    expect(Array.from(out)).toEqual([0, 0, 0, 0]);
  });
});
```

Run: `npx vitest run src/components/RecordingWaveform.test.tsx`
Expected: PASS — 5 tests.

- [ ] **Step 6: Commit**

```bash
git add src/components/RecordingWaveform.tsx src/components/RecordingWaveform.test.tsx
git commit -m "feat(audio): add RecordingWaveform canvas component"
```

---

## Task 3: `useMicAmplitude` hook

**Files:**
- Create: `src/lib/useMicAmplitude.ts`

No automated test — Web Audio's `AudioContext` is too involved to mock through jsdom for this hook's complexity, and the heart of the math (`computeRms`, `RingBuffer`) is already covered by Task 1. Manual QA in Task 5.

- [ ] **Step 1: Implement the hook**

Create `src/lib/useMicAmplitude.ts`:

```ts
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
      analyser.getFloatTimeDomainData(buf);
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
```

- [ ] **Step 2: Type-check**

Run: `npx tsc --noEmit`
Expected: PASS — no errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/useMicAmplitude.ts
git commit -m "feat(audio): add useMicAmplitude hook"
```

---

## Task 4: Wire `RecordCard` to use the new active layout + waveform

**Files:**
- Modify: `src/components/RecordCard.tsx`

Goal: replace the existing `recording`/`paused` branch with the waveform-hero layout. Add `stream` state mirroring `streamRef` so React sees attach/detach.

- [ ] **Step 1: Add `stream` state**

In `src/components/RecordCard.tsx`, add to the existing `useState` block (near `errorMsg`/`elapsedMs`):

```tsx
const [stream, setStream] = useState<MediaStream | null>(null);
```

In `start()`, after `streamRef.current = stream;` (the local `stream` variable from `getUserMedia`), also set the React state. **Rename** the local `stream` to avoid the collision with the new state value:

Find:
```tsx
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
```

Replace with:
```tsx
      const mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = mediaStream;
      setStream(mediaStream);
```

Then update the `MediaRecorder` constructor lines below to use `mediaStream` instead of `stream`:

Find:
```tsx
      const recorder = mime
        ? new MediaRecorder(stream, { mimeType: mime })
        : new MediaRecorder(stream);
```

Replace with:
```tsx
      const recorder = mime
        ? new MediaRecorder(mediaStream, { mimeType: mime })
        : new MediaRecorder(mediaStream);
```

In `stopStreamAndRecorder`, after `streamRef.current = null;`, add:

```tsx
    setStream(null);
```

In `recorder.onstop`, after `streamRef.current = null;`, add:

```tsx
        setStream(null);
```

- [ ] **Step 2: Replace the active-state JSX**

In `RecordCard.tsx`, find the `if (state === "recording" || state === "paused")` branch (the block returning the card with the timer + buttons). Replace the entire returned JSX with:

```tsx
  if (state === "recording" || state === "paused") {
    return (
      <div className={`${cardBase} border-solid border-separator bg-control-background gap-3`}>
        <div className="w-full flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full ${
              state === "recording"
                ? "bg-status-error animate-pulse"
                : "bg-status-warning"
            }`}
          />
          <span className="text-callout font-mono tabular-nums text-label-secondary">
            {formatElapsed(elapsedMs)}
          </span>
          {state === "paused" && (
            <span className="text-footnote text-label-tertiary ml-2">Paused</span>
          )}
        </div>

        <RecordingWaveform
          samples={micAmp.samples}
          state={state === "recording" ? "recording" : "paused"}
          className="h-16"
        />

        <div className="w-full flex items-center gap-2">
          {state === "recording" ? (
            <Button onClick={pause} variant="secondary" size="md" aria-label="Pause">⏸</Button>
          ) : (
            <Button onClick={resume} variant="primary" size="md">▶ Resume</Button>
          )}
          <Button onClick={stop} variant={state === "paused" ? "secondary" : "primary"} size="md">
            ◼ Stop
          </Button>
          <Button
            onClick={discard}
            variant="ghost"
            size="md"
            className="ml-auto hover:bg-status-error/20"
          >
            Discard
          </Button>
        </div>
      </div>
    );
  }
```

The `cardBase` constant currently has `flex flex-col items-center justify-center gap-2 py-10 px-4`. The new layout needs `gap-3` and the items left-aligned. Either change `cardBase` (it's used by all branches) or override on this branch only. Override on this branch only — the new container above already specifies `gap-3` and the existing `cardBase` keeps the rounded border. Add `items-stretch` to override the `items-center` for this branch:

Replace the className above with:
```tsx
      <div className={`${cardBase} !items-stretch !gap-3 border-solid border-separator bg-control-background`}>
```

(The `!` prefixes use Tailwind's important modifier to win over `cardBase`.)

- [ ] **Step 3: Add the imports and hook usage**

At the top of `RecordCard.tsx`, add:

```tsx
import RecordingWaveform from "./RecordingWaveform";
import { useMicAmplitude } from "../lib/useMicAmplitude";
```

Inside the component, after the existing `useState` calls and before the refs, add:

```tsx
  const micAmp = useMicAmplitude(stream, state === "recording");
```

- [ ] **Step 4: Run the dev app and manually verify**

Run: `npm run tauri dev`

Manual checklist:
- Click the mic-record card; grant permission if prompted.
- Speak: bars react to voice; silence shows a baseline.
- Click Pause: bars freeze, dot turns yellow, "Paused" text appears, Resume button is primary.
- Click Resume: bars scroll in from the right again.
- Click Discard: card returns to idle, no waveform.
- Click Stop after a few seconds: card saves, returns to idle.

- [ ] **Step 5: Run frontend tests**

Run: `npx vitest run`
Expected: PASS — all existing + new tests.

- [ ] **Step 6: Commit**

```bash
git add src/components/RecordCard.tsx
git commit -m "feat(record): waveform-hero active layout on mic recording"
```

---

## Task 5: Rust `compute_rms` (TDD)

**Files:**
- Create: `src-tauri/src/capture/amplitude.rs`
- Modify: `src-tauri/src/capture/mod.rs` — declare the module

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/capture/amplitude.rs`:

```rust
//! Per-frame RMS amplitude computation + per-session emitter that pushes
//! samples to the JS side at ~30 fps.

/// Compute RMS over an interleaved or mono float frame, clamped to [0.0, 1.0].
pub fn compute_rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: f32 = data.iter().map(|&s| s * s).sum();
    let rms = (sum / data.len() as f32).sqrt();
    rms.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_silence_is_zero() {
        let data = vec![0.0_f32; 1024];
        assert_eq!(compute_rms(&data), 0.0);
    }

    #[test]
    fn rms_unit_sine_is_one_over_sqrt_two() {
        let n = 1024;
        let data: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * i as f32 / 16.0).sin())
            .collect();
        let r = compute_rms(&data);
        assert!((r - 1.0 / 2.0_f32.sqrt()).abs() < 1e-3, "got {r}");
    }

    #[test]
    fn rms_clamps_above_one() {
        let data = vec![2.0_f32; 8];
        assert_eq!(compute_rms(&data), 1.0);
    }

    #[test]
    fn rms_empty_is_zero() {
        let data: Vec<f32> = Vec::new();
        assert_eq!(compute_rms(&data), 0.0);
    }
}
```

In `src-tauri/src/capture/mod.rs`, find the existing module declarations:

```rust
pub mod cleanup;
pub mod mic_audio;
pub mod mix;
pub mod system_audio;
```

Add `pub mod amplitude;` to that block.

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd src-tauri && cargo test capture::amplitude`
Expected: PASS — 4 tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/capture/amplitude.rs src-tauri/src/capture/mod.rs
git commit -m "feat(capture): add compute_rms helper"
```

---

## Task 6: Rust `AmplitudeEmitter`

**Files:**
- Modify: `src-tauri/src/capture/amplitude.rs`

The emitter holds a bounded `tokio::sync::mpsc` channel. A spawned task drains it at ~30 fps, coalescing by taking the running max within each tick window, and invokes a user-provided callback with the value. Decoupling the callback from `AppHandle` keeps the unit test trivial; the production caller wires `AppHandle::emit` inside the closure.

- [ ] **Step 1: Add the test**

Append to `src-tauri/src/capture/amplitude.rs` (inside the existing `mod tests`):

```rust
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn emitter_coalesces_and_emits_max() {
        let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_cb = collected.clone();
        let emitter = AmplitudeEmitter::spawn(move |rms| {
            collected_cb.lock().unwrap().push(rms);
        });

        // Push a burst within a single ~33ms tick window.
        emitter.push(0.1);
        emitter.push(0.5);
        emitter.push(0.3);

        // Wait long enough for at least one tick to fire.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        drop(emitter);

        let got = collected.lock().unwrap().clone();
        assert!(!got.is_empty(), "emitter never fired");
        // The first emitted value should be the running max (0.5), not 0.1 or 0.3.
        assert_eq!(got[0], 0.5);
    }

    #[tokio::test]
    async fn emitter_drops_when_channel_full() {
        // Allocate emitter and immediately stress with more values than the
        // channel can hold; we should not panic and should still emit the max.
        let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_cb = collected.clone();
        let emitter = AmplitudeEmitter::spawn(move |rms| {
            collected_cb.lock().unwrap().push(rms);
        });

        for _ in 0..1000 {
            emitter.push(0.7);
        }

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        drop(emitter);

        let got = collected.lock().unwrap().clone();
        assert!(got.iter().any(|&v| (v - 0.7).abs() < 1e-3), "expected 0.7 in {got:?}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test capture::amplitude::tests::emitter`
Expected: FAIL — `AmplitudeEmitter` not defined.

- [ ] **Step 3: Implement `AmplitudeEmitter`**

In `src-tauri/src/capture/amplitude.rs`, add above the `#[cfg(test)]` block:

```rust
use std::time::Duration;
use tokio::sync::mpsc;

/// Per-session emitter that buffers RMS samples on a bounded channel and
/// drains them at ~30 fps via a spawned task. Each tick, the running max
/// of buffered samples is forwarded to the user callback.
///
/// The audio callback path uses `try_send` and silently drops on a full
/// channel — the drainer already coalesces, so the only effect of a drop
/// is a slightly less peaky max for that tick.
pub struct AmplitudeEmitter {
    tx: mpsc::Sender<f32>,
    handle: tokio::task::JoinHandle<()>,
}

impl AmplitudeEmitter {
    /// Spawn the drain task. `on_tick` is called at most once per ~33ms with
    /// the running max RMS observed during that window.
    pub fn spawn<F>(on_tick: F) -> Self
    where
        F: Fn(f32) + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<f32>(8);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(33));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip the first immediate tick so we always have a window of data.
            interval.tick().await;

            let mut running_max: f32 = 0.0;
            let mut have_data = false;

            loop {
                tokio::select! {
                    biased;
                    maybe = rx.recv() => {
                        match maybe {
                            Some(v) => {
                                if v > running_max { running_max = v; }
                                have_data = true;
                            }
                            None => break,
                        }
                    }
                    _ = interval.tick() => {
                        if have_data {
                            on_tick(running_max);
                            running_max = 0.0;
                            have_data = false;
                        }
                    }
                }
            }
        });
        Self { tx, handle }
    }

    /// Non-blocking push. Drops the value if the channel is full.
    pub fn push(&self, rms: f32) {
        let _ = self.tx.try_send(rms);
    }
}

impl Drop for AmplitudeEmitter {
    fn drop(&mut self) {
        // Closing the sender ends the drainer's recv loop.
        // (The aborter is a fallback for misbehaving futures.)
        self.handle.abort();
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test capture::amplitude`
Expected: PASS — 6 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture/amplitude.rs
git commit -m "feat(capture): add AmplitudeEmitter with 30fps drain"
```

---

## Task 7: Thread emitter through `mic_audio::start_mic_capture`

**Files:**
- Modify: `src-tauri/src/capture/mic_audio.rs`

- [ ] **Step 1: Update the function signature**

In `src-tauri/src/capture/mic_audio.rs`, replace the `start_mic_capture` signature:

```rust
pub fn start_mic_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
```

(Wrapping in `Arc` so all three cpal-format branches can clone it into their callbacks.)

- [ ] **Step 2: Push RMS in each callback's non-paused path**

In `mic_audio.rs`, in each of the three `device.build_input_stream` branches (F32, I16, U16), after the existing `if paused_cb.load(Ordering::Relaxed) { return; }` guard, push amplitude.

For each branch, you need to clone `amplitude` and capture it in the closure. Add after the existing `let writer_cb = writer.clone();` line:

```rust
    let amplitude_f32 = amplitude.clone();
    let amplitude_i16 = amplitude.clone();
    let amplitude_u16 = amplitude.clone();
```

Then inside the F32 branch's callback:

```rust
            move |data: &[f32], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                amplitude_f32.push(super::amplitude::compute_rms(data));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, data, dev_channels, dev_rate);
            },
```

For the I16 branch, convert to f32 once for the RMS calc and reuse the existing `f32s` conversion in `write_resampled_i16`. Cleanest: compute RMS on a local conversion before the write helper:

```rust
            move |data: &[i16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                amplitude_i16.push(super::amplitude::compute_rms(&f32s));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, &f32s, dev_channels, dev_rate);
            },
```

This requires deleting the old `write_resampled_i16` call inside the I16 branch (we now do the conversion inline).

For the U16 branch:

```rust
            move |data: &[u16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let i16s: Vec<i16> = data.iter().map(|&u| (u as i32 - 32_768) as i16).collect();
                let f32s: Vec<f32> = i16s.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                amplitude_u16.push(super::amplitude::compute_rms(&f32s));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, &f32s, dev_channels, dev_rate);
            },
```

The existing helper `write_resampled_i16` is still defined (unused for now); keep it — `cargo` will warn if it's truly unused. Actually, since we're inlining, delete `write_resampled_i16` entirely from the file:

Find and delete the function:
```rust
fn write_resampled_i16<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    data: &[i16],
    src_channels: u16,
    src_rate: u32,
) {
    let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
    write_resampled_f32(writer, &f32s, src_channels, src_rate);
}
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: builds (existing call sites of `start_mic_capture` will fail; that's fixed in Task 9). To check just this file's syntax independently, ignore the unresolved call-site error for now — the next tasks will resolve it.

If the build error is exclusively the `start_mic_capture` call-site arity mismatch in `mod.rs`, proceed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/capture/mic_audio.rs
git commit -m "feat(capture): mic_audio pushes RMS to AmplitudeEmitter"
```

---

## Task 8: Thread emitter through `system_audio::start_system_capture`

**Files:**
- Modify: `src-tauri/src/capture/system_audio.rs`

- [ ] **Step 1: Update `AudioOutput` to hold the emitter**

In `src-tauri/src/capture/system_audio.rs`, find:

```rust
struct AudioOutput {
    paused: Arc<AtomicBool>,
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
}
```

Replace with:

```rust
struct AudioOutput {
    paused: Arc<AtomicBool>,
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
}
```

- [ ] **Step 2: Compute RMS over the f32 frame and push**

In `did_output_sample_buffer`, after the existing `if self.paused.load(Ordering::Relaxed) { return; }` and after the `audio_buffer_list` check, the inner loop reads bytes 4 at a time as `f32`. Restructure to (a) collect the f32 slice once per buffer, (b) push the RMS, (c) write to the wav from the same slice.

Replace the existing inner loop block:

```rust
        for buf in list.iter() {
            let bytes = buf.data();
            // Each f32 sample is 4 bytes. Truncate any partial trailing bytes.
            let full = bytes.len() - (bytes.len() % 4);
            let mut i = 0;
            while i < full {
                let s = f32::from_ne_bytes([
                    bytes[i],
                    bytes[i + 1],
                    bytes[i + 2],
                    bytes[i + 3],
                ]);
                let clamped = s.clamp(-1.0, 1.0);
                let v = (clamped * f32::from(i16::MAX)) as i16;
                // Errors here are ignored; they'll surface at finalize() time
                // if the writer is in an unrecoverable state.
                let _ = writer.write_sample(v);
                i += 4;
            }
        }
```

With:

```rust
        for buf in list.iter() {
            let bytes = buf.data();
            let full = bytes.len() - (bytes.len() % 4);
            let n = full / 4;
            let mut frame: Vec<f32> = Vec::with_capacity(n);
            let mut i = 0;
            while i < full {
                let s = f32::from_ne_bytes([
                    bytes[i],
                    bytes[i + 1],
                    bytes[i + 2],
                    bytes[i + 3],
                ]);
                frame.push(s);
                i += 4;
            }
            self.amplitude.push(super::amplitude::compute_rms(&frame));
            for s in frame {
                let clamped = s.clamp(-1.0, 1.0);
                let v = (clamped * f32::from(i16::MAX)) as i16;
                let _ = writer.write_sample(v);
            }
        }
```

- [ ] **Step 3: Update `start_system_capture` signature and the construction site**

Find:

```rust
pub fn start_system_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
```

Replace with:

```rust
pub fn start_system_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
```

Find:

```rust
    let output = AudioOutput {
        paused,
        writer: Arc::clone(&writer),
    };
```

Replace with:

```rust
    let output = AudioOutput {
        paused,
        writer: Arc::clone(&writer),
        amplitude,
    };
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/capture/system_audio.rs
git commit -m "feat(capture): system_audio pushes RMS to AmplitudeEmitter"
```

---

## Task 9: Wire emitter through `begin_session` and `start_call_capture`

**Files:**
- Modify: `src-tauri/src/capture/mod.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Add emitter to `CallCaptureSession`**

In `src-tauri/src/capture/mod.rs`, add to the `CallCaptureSession` struct:

```rust
pub struct CallCaptureSession {
    pub id: String,
    pub session_dir: PathBuf,
    pub system_wav: PathBuf,
    pub mic_wav: Option<PathBuf>,
    pub paused: Arc<AtomicBool>,
    pub system_handle: Box<dyn std::any::Any + Send>,
    pub mic_handle: Option<Box<dyn std::any::Any + Send>>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Kept alive for the lifetime of the session; dropped here stops the
    /// 30 fps drain task. Audio callbacks each hold their own `Arc` clone.
    pub _amplitude: Arc<amplitude::AmplitudeEmitter>,
}
```

- [ ] **Step 2: Update `begin_session` to take `AppHandle` and build the emitter**

Find the function signature:

```rust
pub fn begin_session(
    registry: &CallCaptureRegistry,
    app_data_dir: &std::path::Path,
    include_mic: bool,
) -> Result<StartCallCaptureResult, String> {
```

Replace with:

```rust
pub fn begin_session(
    registry: &CallCaptureRegistry,
    app_data_dir: &std::path::Path,
    include_mic: bool,
    app: tauri::AppHandle,
) -> Result<StartCallCaptureResult, String> {
```

After the `let paused = Arc::new(AtomicBool::new(false));` line, build the emitter:

```rust
    let session_id_for_emit = id.clone();
    let app_for_emit = app.clone();
    let amplitude = Arc::new(amplitude::AmplitudeEmitter::spawn(move |rms| {
        use tauri::Emitter;
        let _ = app_for_emit.emit(
            "call-capture-amplitude",
            serde_json::json!({ "sessionId": session_id_for_emit, "rms": rms }),
        );
    }));
```

Update both `start_system_capture` and `start_mic_capture` call sites to pass `amplitude.clone()`:

Find:
```rust
    let system_handle = match system_audio::start_system_capture(&system_wav, paused.clone()) {
```
Replace with:
```rust
    let system_handle = match system_audio::start_system_capture(&system_wav, paused.clone(), amplitude.clone()) {
```

Find:
```rust
        match mic_audio::start_mic_capture(&mic_wav, paused.clone()) {
```
Replace with:
```rust
        match mic_audio::start_mic_capture(&mic_wav, paused.clone(), amplitude.clone()) {
```

In the `let session = CallCaptureSession { ... }` block, add the `_amplitude: amplitude` field:

```rust
    let session = CallCaptureSession {
        id: id.clone(),
        session_dir: dir,
        system_wav,
        mic_wav: mic_path,
        paused,
        system_handle,
        mic_handle,
        sample_rate: system_audio::SAMPLE_RATE,
        channels: system_audio::CHANNELS,
        _amplitude: amplitude,
    };
```

- [ ] **Step 3: Update the Tauri command call site**

In `src-tauri/src/commands.rs`, find:

```rust
    tokio::task::spawn_blocking(move || {
        capture::begin_session(&registry, &app_data_dir, include_mic)
    })
```

Replace with:

```rust
    let app_for_session = app.clone();
    tokio::task::spawn_blocking(move || {
        capture::begin_session(&registry, &app_data_dir, include_mic, app_for_session)
    })
```

(The closure must own `app`, so we clone before moving.)

- [ ] **Step 4: Build and run all backend tests**

Run: `cd src-tauri && cargo build`
Expected: PASS — builds clean.

Run: `cd src-tauri && cargo test`
Expected: PASS — all existing + new tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/capture/mod.rs src-tauri/src/commands.rs
git commit -m "feat(capture): wire AmplitudeEmitter through begin_session"
```

---

## Task 10: `useTauriAmplitude` hook

**Files:**
- Create: `src/lib/useTauriAmplitude.ts`
- Create: `src/lib/useTauriAmplitude.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/lib/useTauriAmplitude.test.tsx`:

```tsx
import { act, render, renderHook } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { describe, expect, it, vi, beforeEach, type Mock } from "vitest";
import { useTauriAmplitude } from "./useTauriAmplitude";

const listenMock = listen as unknown as Mock;

describe("useTauriAmplitude", () => {
  beforeEach(() => {
    listenMock.mockReset();
  });

  it("subscribes on mount and unsubscribes on unmount", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);

    const { unmount } = renderHook(() => useTauriAmplitude("session-1"));
    // Wait for the listen promise to resolve.
    await act(async () => {});
    expect(listenMock).toHaveBeenCalledWith("call-capture-amplitude", expect.any(Function));
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });

  it("only pushes events whose sessionId matches", async () => {
    let handler: ((e: { payload: { sessionId: string; rms: number } }) => void) | null = null;
    listenMock.mockImplementation(async (_name: string, cb: (e: unknown) => void) => {
      handler = cb as typeof handler;
      return () => {};
    });

    const { result } = renderHook(() => useTauriAmplitude("session-1"));
    await act(async () => {});
    expect(handler).not.toBeNull();

    await act(async () => {
      handler!({ payload: { sessionId: "session-1", rms: 0.4 } });
      handler!({ payload: { sessionId: "other", rms: 0.9 } });
      handler!({ payload: { sessionId: "session-1", rms: 0.6 } });
    });

    const { samples } = result.current;
    // The two matching pushes are the most recent two values; the mismatched
    // 0.9 must not appear.
    const last = Array.from(samples.slice(-2));
    expect(last).toEqual([0.4, 0.6]);
    expect(Array.from(samples)).not.toContain(0.9);
  });

  it("returns an empty buffer when sessionId is null", () => {
    const { result } = renderHook(() => useTauriAmplitude(null));
    expect(Array.from(result.current.samples).every((v) => v === 0)).toBe(true);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run src/lib/useTauriAmplitude.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the hook**

Create `src/lib/useTauriAmplitude.ts`:

```ts
import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { RingBuffer } from "./audioAmplitude";

const RING_CAPACITY = 128;

interface AmplitudePayload {
  sessionId: string;
  rms: number;
}

/**
 * Returns a rolling window of normalized [0,1] amplitude samples driven by
 * `call-capture-amplitude` Tauri events filtered to the given `sessionId`.
 * When `sessionId === null`, returns a stable empty buffer.
 */
export function useTauriAmplitude(sessionId: string | null): { samples: Float32Array } {
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
      if (event.payload.sessionId !== sessionId) return;
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
  }, [sessionId]);

  return { samples };
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run src/lib/useTauriAmplitude.test.tsx`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/useTauriAmplitude.ts src/lib/useTauriAmplitude.test.tsx
git commit -m "feat(audio): add useTauriAmplitude hook"
```

---

## Task 11: Wire `RecordCallCard` to use the new active layout + waveform

**Files:**
- Modify: `src/components/RecordCallCard.tsx`

- [ ] **Step 1: Add `sessionId` state**

In `src/components/RecordCallCard.tsx`, find:

```tsx
  const sessionRef = useRef<string | null>(null);
```

Add immediately after:

```tsx
  const [sessionId, setSessionId] = useState<string | null>(null);
```

In `start()`, after `sessionRef.current = result.sessionId;`, add:

```tsx
      setSessionId(result.sessionId);
```

In `discard()`, after `sessionRef.current = null;`, add:

```tsx
    setSessionId(null);
```

In `stop()`, after `sessionRef.current = null;`, add:

```tsx
      setSessionId(null);
```

- [ ] **Step 2: Add hook + waveform import**

At the top of the file, add:

```tsx
import RecordingWaveform from "./RecordingWaveform";
import { useTauriAmplitude } from "../lib/useTauriAmplitude";
```

Inside the component, after the `useState` block, add:

```tsx
  const sysAmp = useTauriAmplitude(sessionId);
```

- [ ] **Step 3: Replace the active layout**

Find the `if (state === "recording" || state === "paused")` branch in `RecordCallCard.tsx` (it returns the card with the timer + Pause/Stop/Discard buttons; today it also renders the `micError` warning). Replace with:

```tsx
  if (state === "recording" || state === "paused") {
    return (
      <div className={`${cardBase} !items-stretch !gap-3 border-solid border-separator bg-control-background`}>
        <div className="w-full flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full ${
              state === "recording"
                ? "bg-status-error animate-pulse"
                : "bg-status-warning"
            }`}
          />
          <span className="text-callout font-mono tabular-nums text-label-secondary">
            {formatElapsed(elapsedMs)}
          </span>
          {state === "paused" && (
            <span className="text-footnote text-label-tertiary ml-2">Paused</span>
          )}
        </div>

        <RecordingWaveform
          samples={sysAmp.samples}
          state={state === "recording" ? "recording" : "paused"}
          className="h-16"
        />

        {micError && (
          <p className="text-footnote text-status-warning">{micError}</p>
        )}

        <div className="w-full flex items-center gap-2">
          {state === "recording" ? (
            <Button onClick={pause} variant="secondary" size="md" aria-label="Pause">⏸</Button>
          ) : (
            <Button onClick={resume} variant="primary" size="md">▶ Resume</Button>
          )}
          <Button onClick={stop} variant={state === "paused" ? "secondary" : "primary"} size="md">
            ◼ Stop
          </Button>
          <Button
            onClick={discard}
            variant="ghost"
            size="md"
            className="ml-auto hover:bg-status-error/20"
          >
            Discard
          </Button>
        </div>
      </div>
    );
  }
```

The `cardBase` constant in this file currently is `"rounded-xl border-2 flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors"`. The `!items-stretch !gap-3` overrides win.

- [ ] **Step 4: Manually QA the system-audio path**

Run: `npm run tauri dev`

Manual checklist:
- Click "Record call". If prompted, grant Screen Recording permission.
- Play a YouTube video / system audio. Bars react to the system audio level.
- Pause: bars freeze, dot turns yellow, "Paused" text appears.
- Resume: bars scroll in from the right.
- Stop: card returns to idle, captured webm shows up in staging.
- Discard: card returns to idle, no file produced.

If `includeMic` is on and the mic was successfully attached, both audio sources contribute to the running max RMS — the waveform reflects both. This matches the spec's "session-level emitter" intent.

- [ ] **Step 5: Run all tests**

Run: `cd /Users/nexus/projects/meister && npx vitest run && (cd src-tauri && cargo test)`
Expected: PASS — all suites green.

- [ ] **Step 6: Commit**

```bash
git add src/components/RecordCallCard.tsx
git commit -m "feat(record): waveform-hero active layout on system-audio recording"
```

---

## Task 12: Final integration QA + cleanup

**Files:**
- None (verification only).

- [ ] **Step 1: Run all tests once more**

Run: `cd /Users/nexus/projects/meister && npx vitest run && (cd src-tauri && cargo test)`
Expected: PASS — full suites green.

- [ ] **Step 2: Type check**

Run: `npx tsc --noEmit`
Expected: PASS — zero errors.

- [ ] **Step 3: Cargo clippy (if configured)**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: PASS — no warnings. If clippy is not configured, skip.

- [ ] **Step 4: Manual end-to-end smoke test**

Run: `npm run tauri dev`

Verify each scenario:
1. Idle → mic record → speak → pause → resume → stop → captured file appears in staging.
2. Idle → mic record → discard.
3. Idle → call record (no mic) → play audio → stop → captured webm appears.
4. Idle → call record (with mic) → speak + audio playback → pause → resume → stop.
5. Mic permission denied: existing error UI shows; no waveform attempted.
6. Screen Recording permission denied: existing `permission-needed` UI; no waveform attempted.
7. Switch projects mid-recording: the existing nav guard fires; card tears down cleanly with no console errors.

- [ ] **Step 5: Confirm no leftover dev artifacts**

Run: `git status`
Expected: clean, except for any new specs/plans you may not have committed yet.

---

## Spec Coverage Self-Check

| Spec section | Implemented in |
|---|---|
| `<RecordingWaveform>` component | Task 2 |
| `useMicAmplitude(stream, active)` hook | Task 3 |
| `useTauriAmplitude(sessionId)` hook | Task 10 |
| `compute_rms` Rust helper | Task 5 |
| `AmplitudeEmitter` Rust struct | Task 6 |
| `mic_audio.rs` integration | Task 7 |
| `system_audio.rs` integration | Task 8 |
| `capture/mod.rs` `CallCaptureSession` + `begin_session` | Task 9 |
| `commands.rs::start_call_capture` AppHandle threading | Task 9 |
| Active-state waveform-hero layout for `RecordCard` | Task 4 |
| Active-state waveform-hero layout for `RecordCallCard` | Task 11 |
| `stream` state in `RecordCard` | Task 4 |
| `sessionId` state in `RecordCallCard` | Task 11 |
| Pause-freeze behavior (mic side) | Task 3 (`active` gate) + Task 4 (passes `state === "recording"`) |
| Pause-freeze behavior (system side) | Task 8 (Rust gates push behind `paused` flag) |
| Drop accent-tint background | Tasks 4 & 11 |
| Manual QA matrix | Task 12 |
| Rust unit tests (rms, emitter) | Tasks 5–6 |
| Frontend unit tests (computeRms, RingBuffer, downsampleMax, useTauriAmplitude) | Tasks 1, 2, 10 |
