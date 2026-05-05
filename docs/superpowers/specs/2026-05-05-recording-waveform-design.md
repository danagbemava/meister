# Recording Waveform — Design

**Status:** spec
**Date:** 2026-05-05
**Scope:** replace the elapsed-time-only display during active recording with a live audio waveform, shared across both mic recording (`RecordCard`) and system-audio recording (`RecordCallCard`).

## Motivation

Today, an active recording shows a status dot and a numeric timer — nothing reflects that audio is actually being captured. The user has to trust the dot. Adding a live waveform (a) confirms audio is reaching the recorder, (b) gives the recording state a stronger visual identity, and (c) brings the recording cards closer to the macOS-native feel the chrome-basics work has been pursuing.

The companion media-playback feature is intentionally split out into its own spec; the two share no code.

## Architecture

A single shared renderer is fed amplitude samples from two source-specific hooks:

```
RecordCard.tsx (mic)             RecordCallCard.tsx (system audio)
       │                                 │
useMicAmplitude(stream)        useTauriAmplitude(sessionId)
       │                                 │
       └────────► <RecordingWaveform> ◄──┘
                  (canvas + ring buffer)

           Web Audio                    Tauri event
           AnalyserNode                 "call-capture-amplitude"
           (in JS)                      (from Rust capture loops)
```

Amplitude is computed at the audio source (Web Audio for mic, the Rust capture callbacks for system audio), so paused / discarded sessions stop emitting naturally when their stream is paused or torn down.

## Components

### `<RecordingWaveform>` — `src/components/RecordingWaveform.tsx`

Presentational. Pure render of its props.

```ts
interface Props {
  /** Normalized [0,1] amplitude samples, oldest → newest. */
  samples: Float32Array;
  state: "recording" | "paused";
  className?: string;
}
```

- Canvas-based draw inside a `useEffect` keyed on the `samples` reference.
- Bars: 2px wide, 1px gap. Bar height = `Math.max(2, sample * canvasHeight)` so silence still shows a baseline.
- Color: `--accent` when `state === "recording"`, `--label-tertiary` when `state === "paused"`.
- Bar count = `floor(width / 3)`. Samples shorter than the bar count are right-aligned (newest at the right edge); longer buffers are downsampled to fit by taking max-of-window per bar.
- No internal state.

### `useMicAmplitude(stream, active)` — `src/lib/useMicAmplitude.ts`

```ts
function useMicAmplitude(stream: MediaStream | null, active: boolean): { samples: Float32Array };
```

- On `stream` attach: creates `AudioContext`, `AnalyserNode` (`fftSize = 1024`), connects `createMediaStreamSource(stream)`.
- `requestAnimationFrame` loop: each tick reads time-domain data via `getFloatTimeDomainData`, computes RMS over the buffer.
- **`active` gate**: when `active === true`, pushes the RMS value into a fixed-size ring buffer (capacity 128). When `active === false`, the loop still runs but skips the push — this freezes the visible buffer on pause without tearing down the `AudioContext`.
- Throttles pushes to ~30 fps; if RAF fires faster, intermediate ticks are skipped for the buffer write but the loop keeps running.
- On `stream` change to `null` or unmount: closes the `AudioContext`, returns a stable empty buffer.
- Failure to construct `AudioContext` is caught; hook returns an empty buffer and recording proceeds normally.

Callers pass `active = state === "recording"` so paused state freezes the buffer.

### `useTauriAmplitude(sessionId: string | null)` — `src/lib/useTauriAmplitude.ts`

Returns `{ samples: Float32Array }`.

- Subscribes to the Tauri event `call-capture-amplitude`. Payload shape: `{ sessionId: string, rms: number }`.
- Filters payloads where `payload.sessionId === sessionId`; pushes `payload.rms` into a ring buffer of capacity 128.
- Cleans up the listener on `sessionId` change or unmount.
- Listener-attach failure is caught; hook returns an empty buffer.

### Rust additions — `src-tauri/src/capture/`

New module: `amplitude.rs`.

- `pub fn compute_rms(data: &[f32]) -> f32` — computes RMS over an interleaved or mono float frame, clamps to `[0.0, 1.0]`.
- `pub struct AmplitudeEmitter` — per-session emitter holding:
  - A bounded `tokio::sync::mpsc` channel (capacity ~8).
  - A spawned task that drains the channel at ~30 fps (`tokio::time::interval(Duration::from_millis(33))`), coalescing by taking the running max within each tick window, and emits `call-capture-amplitude { sessionId, rms }` via a held `AppHandle`.
  - On drop: aborts the spawned task.

Edits:

- `mic_audio.rs` — in the existing `move |data: &[f32], _|` cpal callback, also call `emitter.push(compute_rms(data))` **when the session is not paused**, gated by the same `paused` flag that already gates wav writes.
- `system_audio.rs` — in `did_output_sample_buffer`, after the existing f32→i16 path, call `emitter.push(compute_rms(slice))` on the interleaved f32 slice, **gated by the same `paused` flag**.
- `capture/mod.rs` — `begin_session` constructs an `AmplitudeEmitter` (cloning `AppHandle` and threading the session id) and stores it in the `Session` struct alongside the existing writers. `set_paused` already toggles the per-session `paused` flag observed by both audio callbacks; no new wiring needed beyond reading it before `emitter.push`. `discard_session` and `finalize_session` drop the emitter, which stops the task.

The audio callbacks must remain non-blocking; `emitter.push` uses `try_send` and silently drops on a full channel.

## Card integration

### Active layout (both cards)

Once `state` enters `recording` or `paused`, the card uses this layout — the waveform is the focal point, controls have explicit weight:

```
┌──────────────────────────────────────────────┐
│                                              │
│   ● 00:42                                    │
│                                              │
│   ▂▃▅▆▇▆▅▄▃▂▁▁▂▃▅▆▇▆▅▃▂▁▁▂▃▅                 │
│                                              │
│                                              │
│   ┌────┐  ┌──────────┐                       │
│   │ ⏸  │  │  ◼ Stop  │           Discard     │
│   └────┘  └──────────┘                       │
│                                              │
└──────────────────────────────────────────────┘
```

Specifics:

- **Meta strip (top-left):** small pulsing dot (`--accent`, animate-pulse) + monospace timer in `text-callout`. The current "huge timer" treatment goes away.
- **Waveform:** fills card width, 64–72px tall.
- **Card chrome:** standard `bg-control-background` with `border-separator`. **Drop the existing `bg-accent/10` tint** — the waveform plus pulsing dot signal the active state without a theme swap, which fits the macOS-native arc better.
- **Controls (bottom row, left-aligned):**
  - Pause: icon-only secondary `Button` (~32×32).
  - Stop: primary `Button` with stop glyph + label, sized larger.
  - Discard: ghost `Button` floated to the right edge, far from the primary action to avoid mis-clicks.

### Paused state

- Dot turns `--status-warning`, animation stops.
- Waveform re-renders with `state="paused"` → bars shift to `--label-tertiary`.
- Small "Paused" caption appears under the timer.
- The icon-only Pause button is replaced by a primary Resume button; Stop becomes secondary while paused.

### Driving the waveform

- `RecordCard` adds a `stream` state value alongside the existing `streamRef` (set whenever the ref is set or cleared, so renders see attach/detach). Calls `useMicAmplitude(stream, state === "recording")`. The hook attaches once the stream exists post-`getUserMedia` and returns an empty buffer before then.
- `RecordCallCard` adds a `sessionId` state value alongside `sessionRef`. Calls `useTauriAmplitude(sessionId)`. The Tauri event is only emitted by the Rust side from non-paused callbacks, so the buffer freezes naturally on pause without the hook needing an `active` gate.

### Idle / saving / error / permission-needed states

Unchanged. The waveform layout is only used inside the existing `recording`/`paused` branch.

## Concurrency

`ProjectView` only ever has one active capture card at a time (the recording-in-progress nav-guard in the existing code already enforces this). The spec assumes this invariant; two amplitude streams cannot coexist.

## Error handling & edge cases

| Case | Behavior |
|------|----------|
| `AudioContext` create fails | `useMicAmplitude` catches, returns empty buffer; renderer shows flat baseline; recording continues. |
| Tauri event listener attach fails | `useTauriAmplitude` catches, returns empty buffer; recording continues. |
| Rust emitter channel full | `try_send` drops the new value; the drain task already coalesces by taking running max. |
| Pause | Hook stops pushing; existing buffer freezes; renderer re-renders desaturated. |
| Resume | New samples scroll in from the right; existing buffer continues normally. |
| Discard mid-recording | Existing teardown path (`stopStreamAndRecorder` / `discardSession`) runs first → hook returns empty buffer → next paint clears. |
| Permission states (`permission-needed`, `error`, `idle`, `saving`) | Waveform is not rendered. Those branches keep their existing layouts. |

## Testing

### Rust (`capture/amplitude.rs`)

- `compute_rms` on a unit-amplitude sine wave returns ~`1 / sqrt(2)` (within 1e-3).
- `compute_rms` on silence returns `0.0`.
- `compute_rms` on a clipped/over-unit signal clamps to `1.0`.

### Frontend (Vitest)

- `useMicAmplitude` ring buffer pushes and wraps correctly given a mocked `AnalyserNode.getFloatTimeDomainData`.
- `useMicAmplitude` closes its `AudioContext` on unmount.
- `useTauriAmplitude` only pushes events whose `sessionId` matches; ignores others.
- `useTauriAmplitude` removes its listener on `sessionId` change and on unmount.

### Manual QA

- Mic recording: bars react to speech, freeze on pause, scroll on resume, clear on discard.
- Call recording: bars react to system audio (e.g., a YouTube video), behave identically.
- Mic permission denied → existing error state shown, no waveform attempted.
- System-audio permission denied → existing `permission-needed` state shown, no waveform attempted.
- Switching projects mid-recording: existing nav guard fires; waveform tears down with the rest of the capture state.

## Files affected

New:
- `src/components/RecordingWaveform.tsx`
- `src/lib/useMicAmplitude.ts`
- `src/lib/useTauriAmplitude.ts`
- `src-tauri/src/capture/amplitude.rs`

Edited:
- `src/components/RecordCard.tsx` — new active-layout, mounts waveform, drops accent tint.
- `src/components/RecordCallCard.tsx` — same active-layout, drops accent tint.
- `src-tauri/src/capture/mic_audio.rs` — push RMS into emitter from cpal callback.
- `src-tauri/src/capture/system_audio.rs` — push RMS into emitter from SCStream callback.
- `src-tauri/src/capture/mod.rs` — construct/store/drop `AmplitudeEmitter` per session; thread `AppHandle` through.

## Out of scope

- Playback waveforms (post-recording scrubber). Covered by the separate media-playback spec.
- Persisting amplitude data to disk for any later visualization.
- Per-source level meters in the multi-source case (irrelevant here — recording is single-source per active session).
- Configurability of bar pitch, fps, ring-buffer size — sane defaults only.
