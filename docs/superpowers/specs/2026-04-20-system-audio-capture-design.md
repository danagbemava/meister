# System Audio Capture — Design

**Status:** Draft
**Date:** 2026-04-20

## Problem

Meister's mic-capture feature records only the physical microphone — useless for capturing Zoom, Google Meet, WhatsApp, or any remote call where the other participants' voices come out of the speakers, not into the mic. Users currently need a separate tool (Audio Hijack, Loopback, OBS) to capture call audio, then drop the resulting file into Meister.

## Goals

- Record all system audio output on macOS directly inside Meister, no third-party drivers.
- Optionally mix the mic into the same file so a single capture contains both sides of a call.
- Full media controls (Start / Pause / Resume / Stop / Discard) — parity with mic-capture.
- Captured files land in ProjectView's staging area, same code path as dropped files and mic captures.
- Orphan temp files (from crashes or discards) get swept at startup.

## Non-goals

- **Per-app audio isolation** (SCStream supports it; needs a picker UI — deferred).
- **Screen video recording.** Audio stream only. SCStream can deliver frames; we discard them.
- **Live input level meter / waveform preview / trimming.**
- **Windows / Linux.** ScreenCaptureKit is macOS-only; Meister is macOS-first.

## Architecture

Three moving parts:

1. **Rust capture core** — a new module `src-tauri/src/capture/` that owns SCStream + cpal sessions, writes two PCM WAV tracks to a per-session temp dir, and finalizes on Stop by invoking ffmpeg to mix+encode to WebM/Opus. Produces the same WebM file format the mic-capture feature already ships, so the existing staging list shows one consistent row type.
2. **Five new Tauri commands** — `start_call_capture`, `pause_call_capture`, `resume_call_capture`, `stop_call_capture`, `discard_call_capture`. State held in `Arc<Mutex<HashMap<SessionId, CallCaptureSession>>>` registered in Tauri state.
3. **New `RecordCallCard` React component** — sibling to the existing `RecordCard`, with a third slot in `ProjectView`'s Sources row. Mirrors `RecordCard`'s state machine but adds a "Include microphone" toggle (default ON) and a "Permission needed" state for Screen Recording grants.

## Capture core (Rust)

### Dependencies (new)

```toml
screencapturekit = "0.3"        # SCStream wrapper
cpal             = "0.15"       # cross-platform mic capture
```

`hound` is already in the project (used by whisper transcription) — reused for PCM WAV writes.

### Module layout

```
src-tauri/src/capture/
├── mod.rs                 # CallCaptureSession + registry
├── system_audio.rs        # SCStream wiring → system.wav writer
├── mic_audio.rs           # cpal wiring → mic.wav writer
└── mix.rs                 # Stop handler: ffmpeg amix/encode
```

### Session state

```rust
pub struct CallCaptureSession {
    pub id: String,                     // uuid
    pub session_dir: PathBuf,           // <app_data>/tmp/call-captures/<id>/
    pub system_writer: Arc<Mutex<WavWriter>>,
    pub mic_writer: Option<Arc<Mutex<WavWriter>>>,
    pub sc_stream_handle: SCStreamHandle, // keeps the stream alive
    pub mic_stream_handle: Option<cpal::Stream>,
    pub paused: Arc<AtomicBool>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct CallCaptureRegistry {
    pub sessions: Mutex<HashMap<String, CallCaptureSession>>,
}
```

The `paused` flag is checked inside both the SCStream and cpal callbacks; when true, frames are dropped instead of written. Cheap, atomic, lock-free.

Registered in Tauri state next to `Store` and the transcription `CancelRegistry`:

```rust
app.manage(Arc::new(capture::CallCaptureRegistry::default()));
```

### Data formats

- SCStream delivers `CMSampleBuffer` → extracted to interleaved `f32` (typical) or `i16` PCM, 48 kHz stereo by default.
- cpal mic delivers PCM in the device's native format. Resample to match SCStream's format before writing (use `rubato` or a simple linear resample; mic is usually already 48 kHz so no-op typical).
- Both writers produce 48 kHz stereo 16-bit WAV (`WavSpec` with `channels: 2, sample_rate: 48000, bits_per_sample: 16, sample_format: Int`).

### Stop + mix

```rust
pub fn finalize_session(session: CallCaptureSession, captures_dir: &Path) -> Result<PathBuf, String>
```

Closes both writers (`WavWriter::finalize()` writes the header), then:

**If only system captured:**
```
ffmpeg -i system.wav -c:a libopus -b:a 96k <final>.webm
```

**If both captured:**
```
ffmpeg -i system.wav -i mic.wav \
  -filter_complex "[0:a][1:a]amix=inputs=2:duration=longest:dropout_transition=0,aresample=48000[out]" \
  -map "[out]" \
  -c:a libopus -b:a 96k \
  <final>.webm
```

`<final>` = `<app_data>/captures/capture-YYYYMMDD-HHMMSS-<xxxxxxxx>.webm` — same naming convention as mic-capture.

After ffmpeg success, delete the session temp dir. On ffmpeg failure, keep the temp dir so Retry can re-attempt (exposed as a retry case on the Tauri command).

### Orphan temp cleanup

New function `cleanup_orphaned_call_temps(captures_tmp_dir: &Path, active_ids: &HashSet<String>) -> Result<usize, String>` — deletes any child directory whose name isn't in the active-session set. Called at app startup with an empty active-session set (no sessions survive a relaunch).

## Tauri commands

```rust
#[tauri::command]
pub async fn start_call_capture(
    app: AppHandle,
    registry: State<'_, Arc<CallCaptureRegistry>>,
    include_mic: bool,
) -> Result<StartCaptureResult, String> { ... }

#[derive(Serialize)]
pub struct StartCaptureResult {
    pub session_id: String,
    pub system_ok: bool,
    pub mic_ok: bool,            // false if includeMic was true but mic auth/init failed
    pub mic_error: Option<String>,
}
```

`system_ok = false` means Screen Recording not authorized; frontend flips to "Permission needed" state and skips starting the session.

Other commands take a `session_id: String` and operate on the matching session. `stop_call_capture` returns the final path, `discard_call_capture` returns `()`.

## UI

### Sources row

`ProjectView`'s Sources row becomes 3 cards on wide screens:

```
┌────────────────┐  ┌────────────────┐  ┌──────────────────────────┐
│ 🎬             │  │ 🎙             │  │ 🔊                       │
│ Drop files     │  │ Record audio   │  │ Record call              │
│                │  │                │  │ ☑ Include microphone     │
└────────────────┘  └────────────────┘  └──────────────────────────┘
```

Grid: `grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3`.

### `RecordCallCard` component

Layout mirrors `RecordCard`:

| State | Visual | Controls |
|---|---|---|
| **Idle** | 🔊 + "Record call" + "☑ Include microphone" checkbox | Click → Start |
| **Permission needed** | Amber border + copy + guidance | Open Settings · Retry |
| **Recording** | Pulsing red dot + timer + badge ("System + Mic" or "System only") | ⏸ Pause · ⏹ Stop · ✕ Discard |
| **Paused** | Amber dot + frozen timer | ▶ Resume · ⏹ Stop · ✕ Discard |
| **Mixing** | Spinner + "Mixing tracks…" | (none) |
| **Saving** | Spinner + "Saving…" | (none) |
| **Error** | Red card + message | Retry |

"Include microphone" checkbox is interactive only in Idle. Locked once capture starts.

Passes `onPicked(paths)` and `onActiveChange(discard)` upward, same contract as `RecordCard`. The navigation guard lifted to `App.tsx` during mic-capture already handles "discard on leave" correctly — no change needed.

If `includeMic` was true but mic init failed (`mic_ok: false` in the Start result), card shows an amber sub-banner: "Microphone unavailable — recording system audio only." Capture proceeds.

### Permission card state

On Start, if `start_call_capture` returns `system_ok: false`:

- **Open Settings** button → invokes `tauri-plugin-shell` with `open "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"`.
- **Retry** button → re-invokes `start_call_capture`.
- Copy includes: _"macOS requires you to relaunch Meister after granting Screen Recording permission."_ (Known TCC limitation.)

## macOS permissions

Add to `src-tauri/Info.plist`:

```xml
<key>NSScreenCaptureUsageDescription</key>
<string>Meister captures system audio to record calls and meetings for transcription.</string>
```

`NSMicrophoneUsageDescription` is already present from mic-capture.

No prompt appears automatically on first SCStream creation; macOS returns an authorization error or silently gives zero frames. We detect and surface via the Permission Needed state.

## Error handling

| Error | Surface |
|---|---|
| Screen Recording not authorized | Permission Needed state (with Open Settings + Retry) |
| Mic not authorized (and `includeMic` on) | Amber sub-banner; capture proceeds system-only |
| SCStream yields no frames for 2s after start | Treat as unauthorized; flip to Permission Needed |
| cpal default-input-device query fails | Same graceful degrade as mic-not-authorized |
| ffmpeg mix/encode failure | Error card + stderr tail; temp WAVs preserved for Retry |
| Disk full during capture | hound write error → session marked failed; error card |
| App crash mid-capture | Temp dir orphaned; startup sweep removes it |
| Mid-capture navigation | Existing `App.tsx` guard confirms and calls `discard` |

## Testing

**Rust unit tests** (no live audio required):
- `mix_single_source_passthrough` — one WAV fixture → ffmpeg re-encodes to Opus → output exists, non-empty.
- `mix_two_sources_amix` — two PCM WAV fixtures → ffmpeg amix runs → output duration ≥ max(input duration).
- `cleanup_orphaned_call_temps_deletes_inactive_session_dirs` — seed temp dir with two session folders, mark one active, cleanup; only the inactive one is deleted.
- `session_registry_roundtrip` — insert/remove session, assert HashMap behavior, assert `paused` flag is observable from another thread.

**Rust integration (gated `#[ignore]`, manual)**:
- `captures_system_audio_two_seconds_end_to_end` — requires Screen Recording permission; verifies a 2s capture produces a non-empty final WebM.

**Manual:**
1. First-run Screen Recording permission flow: fresh install → Record call → Permission Needed card → Open Settings → grant → relaunch Meister → Record call → works.
2. Podcast capture (system-only): play a podcast, toggle off "Include microphone", Record call, Stop after 10s. Drop into staging, Create, Run pipeline — transcript matches podcast.
3. 1-on-1 call capture (system + mic): use phone as second participant, speak and let them speak, Stop. Transcript has both voices.
4. Pause / Resume produces one continuous file without the paused audio.
5. Discard mid-capture: no file appears in `<app_data>/captures/`, temp dir deleted.
6. Mid-capture navigation confirms and discards.
7. Revoke Screen Recording permission in System Settings → Record call → Permission Needed card.
8. Crash recovery: `kill -9` mid-capture → relaunch → `<app_data>/tmp/call-captures/` is empty.

## Future work

- Per-app audio isolation via SCStream's content-filter API (pick "Chrome" → Zoom tab's audio only).
- Screen video recording (use the frame stream we currently discard).
- Live input level meter.
- Windows system audio via WASAPI loopback; Linux via PulseAudio monitor device. Requires parallel capture modules and dep surface on those platforms.
- Post-capture waveform preview + non-destructive trim.

## Open questions

None.
