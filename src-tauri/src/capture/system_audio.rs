//! SCStream wiring for macOS system audio capture.
//!
//! Produces a 48 kHz / stereo / 16-bit signed PCM WAV file for the duration
//! the returned handle is kept alive. Dropping the handle stops the stream
//! and finalizes the WAV.
//!
//! Pause/Resume is implemented by toggling the `paused` flag the caller owns.
//! While paused, audio callbacks drop incoming frames — we never buffer them.
//!
//! ## Crate API note
//!
//! `screencapturekit` 1.x exposes a trait-based handler model:
//!
//! - `SCShareableContent::get()` enumerates displays. It fails with
//!   `SCError::NoShareableContent` when Screen Recording permission is missing.
//! - `SCContentFilter::create().with_display(d).with_excluding_windows(&[]).build()`
//!   builds a filter that captures everything on the display (audio follows
//!   from the display being the capture target).
//! - `SCStreamConfiguration` has `with_captures_audio(true)`, `with_sample_rate`,
//!   `with_channel_count` setters that work on the default feature set.
//! - `SCStream::new(&filter, &config)` + `add_output_handler(handler, Audio)`
//!   + `start_capture()` is the full wiring.
//! - `start_capture()` returns `SCError::CaptureStartFailed`; the inner
//!   `SCStreamError { code: UserDeclined | MissingEntitlements, .. }` variant
//!   is the TCC-denied signal.
//!
//! Audio samples arrive as `CMSampleBuffer`s with an `AudioBufferList`. The
//! platform delivers **interleaved f32** at the configured rate — a single
//! buffer whose byte length is `frames * channels * 4`. We convert to i16
//! and write into the hound `WavWriter`.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hound::WavWriter;
use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::error::{SCError, SCStreamErrorCode};
use screencapturekit::prelude::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutputTrait,
    SCStreamOutputType,
};
use screencapturekit::stream::delegate_trait::StreamCallbacks;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const BITS: u16 = 16;

pub fn wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: BITS,
        sample_format: hound::SampleFormat::Int,
    }
}

/// Shared writer slot: holds the file until the first sample arrives, then
/// becomes the live `WavWriter` configured from the actually-delivered format.
/// `with_sample_rate`/`with_channel_count` on `SCStreamConfiguration` are hints
/// the platform can ignore, so we wait to see what we're actually getting before
/// committing the WAV header.
struct PendingWriter {
    /// `Some` until the first sample buffer arrives. Moved into the `WavWriter`
    /// when it's created; `None` afterwards (or if init failed).
    file: Option<BufWriter<File>>,
    /// `Some` once the first buffer's format description has been read and the
    /// header committed. All subsequent samples go here.
    writer: Option<WavWriter<BufWriter<File>>>,
    /// Set once after a failed init so we don't spam the log on every callback.
    init_failed: bool,
}

/// Once-latch that surfaces an *unexpected* SCStream stop to the caller.
///
/// SCStream dies asynchronously when the display it's bound to sleeps or
/// locks, when replayd restarts, etc. The delegate can fire both
/// `did_stop_with_error` and `stream_did_stop` for one event, and it also
/// fires during our own teardown — this latch collapses all of that into at
/// most one callback, and none at all once teardown has begun.
struct StopNotifier {
    /// Set by `SystemAudioHandle::drop` before the stream is released, so a
    /// stop observed during expected teardown is not reported as a failure.
    teardown: Arc<AtomicBool>,
    notified: AtomicBool,
    callback: Box<dyn Fn(Option<String>) + Send + Sync>,
}

impl StopNotifier {
    fn notify(&self, error: Option<String>) {
        if self.teardown.load(Ordering::Relaxed) {
            return;
        }
        if self.notified.swap(true, Ordering::Relaxed) {
            return;
        }
        eprintln!(
            "[capture/system_audio] SCStream stopped mid-session: {}",
            error.as_deref().unwrap_or("(no error reported)"),
        );
        (self.callback)(error);
    }
}

/// Opaque handle returned to the caller. `Drop` finalizes the WAV writer(s) and
/// releases the SCStream. We box this as `dyn Any + Send` in the public API
/// so the registry stays agnostic across capture backends.
struct SystemAudioHandle {
    // SCStream's Drop stops the capture via sc_stream_release. We keep it
    // alive in the handle so the stream runs for the session's lifetime.
    _stream: SCStream,
    writer: Arc<Mutex<PendingWriter>>,
    /// Present when ScreenCaptureKit-native microphone capture is enabled — the
    /// `SCStreamOutputType::Microphone` handler writes here. Finalized on drop
    /// alongside the system writer.
    mic_writer: Option<Arc<Mutex<PendingWriter>>>,
    /// Shared with the stream delegate; set before release so a delegate
    /// callback fired by our own teardown isn't reported as a mid-session stop.
    teardown: Arc<AtomicBool>,
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        self.teardown.store(true, Ordering::Relaxed);
        // Finalize the WAVs first so the headers are written while any in-flight
        // callback that still held the mutex has a chance to drain. The
        // stream release in SCStream::drop happens after this scope ends.
        for slot in [Some(&self.writer), self.mic_writer.as_ref()].into_iter().flatten() {
            if let Ok(mut guard) = slot.lock() {
                if let Some(w) = guard.writer.take() {
                    let _ = w.finalize();
                }
                // The BufWriter<File> drops here if the writer was never created
                // (no samples received). File handle closes normally.
            }
        }
    }
}

/// Output handler that drops frames while paused, otherwise reads the actual
/// delivered audio format on first sample, writes samples to the lazily-created
/// WAV, and handles both interleaved and non-interleaved (planar) buffer layouts.
///
/// One `AudioOutput` is registered per output type we care about: the system
/// audio tap (`accept = Audio`) and, when enabled, the SCStream-native
/// microphone tap (`accept = Microphone`). `label` distinguishes them in logs.
struct AudioOutput {
    paused: Arc<AtomicBool>,
    writer: Arc<Mutex<PendingWriter>>,
    /// `Some` only for the meter-driving system tap; `None` for the mic tap so
    /// it doesn't double-count into the UI amplitude meter.
    amplitude: Option<Arc<super::amplitude::AmplitudeEmitter>>,
    /// Which output type this handler consumes; buffers of any other type are
    /// ignored (the stream multiplexes Audio + Microphone to all handlers).
    accept: SCStreamOutputType,
    label: &'static str,
    /// First-buffer diagnostic latch, per handler instance.
    logged: AtomicBool,
}

impl SCStreamOutputTrait for AudioOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        // The stream delivers every output type to every handler; take only ours.
        if of_type != self.accept {
            return;
        }
        if self.paused.load(Ordering::Relaxed) {
            return;
        }

        let Some(list) = sample_buffer.audio_buffer_list() else {
            return;
        };

        // Diagnostic: log the actual format of the first buffer received on this
        // handler, exactly once. The `with_sample_rate()` / `with_channel_count()`
        // we pass to SCStreamConfiguration are requests, not guarantees — this
        // surfaces what the platform actually delivered, per tap.
        if !self.logged.swap(true, Ordering::Relaxed) {
            let bufs: Vec<_> = list.iter().collect();
            let first_bytes = bufs.first().map(|b| b.data().len()).unwrap_or(0);
            let n_samples = sample_buffer.num_samples();
            if let Some(fd) = sample_buffer.format_description() {
                eprintln!(
                    "[capture/system_audio] FIRST BUFFER [{}]: requested {} Hz × {} ch; \
                     delivered rate={:?} Hz, channels={:?}, bits={:?}, \
                     bytes_per_frame={:?}, is_float={}, format_flags={:?}; \
                     ABL buffers={}, first_buffer_bytes={}, num_samples={}",
                    self.label, SAMPLE_RATE, CHANNELS,
                    fd.audio_sample_rate(),
                    fd.audio_channel_count(),
                    fd.audio_bits_per_channel(),
                    fd.audio_bytes_per_frame(),
                    fd.audio_is_float(),
                    fd.audio_format_flags(),
                    bufs.len(),
                    first_bytes,
                    n_samples,
                );
            } else {
                eprintln!(
                    "[capture/system_audio] FIRST BUFFER [{}]: no format_description; \
                     requested {} Hz × {} ch; ABL buffers={}, first_buffer_bytes={}, \
                     num_samples={}",
                    self.label, SAMPLE_RATE, CHANNELS, bufs.len(), first_bytes, n_samples,
                );
            }
        }

        let mut guard = match self.writer.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        // Lazy WAV initialization: the first sample buffer's format_description
        // tells us the actual delivered sample rate and channel count. Build the
        // header from those — not from what we requested. Downstream `aresample`
        // in mix.rs converts to 48 kHz before mixing.
        if guard.writer.is_none() && !guard.init_failed {
            let spec = match build_wav_spec_from(&sample_buffer) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[capture/system_audio] cannot read delivered format: {e}; capture WAV will be empty");
                    guard.init_failed = true;
                    return;
                }
            };
            let Some(file) = guard.file.take() else {
                guard.init_failed = true;
                return;
            };
            match WavWriter::new(file, spec) {
                Ok(w) => {
                    eprintln!(
                        "[capture/system_audio] writing WAV at delivered format: {} Hz × {} ch",
                        spec.sample_rate, spec.channels
                    );
                    guard.writer = Some(w);
                }
                Err(e) => {
                    eprintln!("[capture/system_audio] WavWriter init failed: {e}");
                    guard.init_failed = true;
                    return;
                }
            }
        }
        let Some(writer) = guard.writer.as_mut() else {
            return;
        };

        // Collect buffer slices once so we can branch on layout. SCStream usually
        // delivers a single buffer (interleaved or mono), but planar multi-channel
        // delivery (one AudioBuffer per channel) is a valid CoreAudio layout and
        // is signalled by `bufs.len() > 1`. In that case, we must interleave
        // before writing so hound's frame layout matches the WAV header.
        let bufs: Vec<&[u8]> = list.iter().map(|b| b.data()).collect();

        let frame: Vec<f32> = if bufs.len() <= 1 {
            let bytes = bufs.first().copied().unwrap_or(&[]);
            let full = bytes.len() - (bytes.len() % 4);
            let mut out = Vec::with_capacity(full / 4);
            let mut i = 0;
            while i < full {
                out.push(f32::from_ne_bytes([
                    bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3],
                ]));
                i += 4;
            }
            out
        } else {
            // Planar: take the minimum sample count across channels (defensive
            // against unequal buffer lengths) and interleave channel-by-channel.
            let samples_per_ch = bufs.iter().map(|b| b.len() / 4).min().unwrap_or(0);
            let mut out: Vec<f32> = Vec::with_capacity(samples_per_ch * bufs.len());
            for sample_idx in 0..samples_per_ch {
                for ch_bytes in &bufs {
                    let i = sample_idx * 4;
                    out.push(f32::from_ne_bytes([
                        ch_bytes[i], ch_bytes[i + 1], ch_bytes[i + 2], ch_bytes[i + 3],
                    ]));
                }
            }
            out
        };

        if let Some(amp) = &self.amplitude {
            amp.push(super::amplitude::compute_rms(&frame));
        }
        for s in frame {
            let clamped = s.clamp(-1.0, 1.0);
            let v = (clamped * f32::from(i16::MAX)) as i16;
            let _ = writer.write_sample(v);
        }
    }
}

/// Build a `WavSpec` from the first sample buffer's format description. Falls
/// back to no-op errors when fields the platform should always populate are
/// missing — those failures get logged once and the WAV is left empty (the
/// capture won't crash the recording pipeline).
fn build_wav_spec_from(sb: &CMSampleBuffer) -> Result<hound::WavSpec, String> {
    let fd = sb.format_description().ok_or_else(|| "no format_description".to_string())?;
    let rate = fd.audio_sample_rate().ok_or_else(|| "format_description has no sample_rate".to_string())?;
    let channels = fd.audio_channel_count().ok_or_else(|| "format_description has no channel_count".to_string())?;
    Ok(hound::WavSpec {
        channels: channels as u16,
        sample_rate: rate as u32,
        bits_per_sample: BITS,
        sample_format: hound::SampleFormat::Int,
    })
}

/// Start a background SCStream that mixes every process's audio into a single
/// stereo 48 kHz i16 WAV file. Returns a handle whose `Drop` tears everything
/// down and finalizes the file. Returns `Err("not-authorized")` when Screen
/// Recording permission has not been granted.
///
/// `on_stopped` fires at most once, and only if the stream dies while the
/// handle is still alive (display sleep, replayd restart, …) — never for the
/// stop caused by dropping the handle. The optional message is SCK's error
/// description.
pub fn start_system_capture(
    wav_path: &Path,
    mic_wav_path: Option<&Path>,
    paused: Arc<AtomicBool>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
    on_stopped: Box<dyn Fn(Option<String>) + Send + Sync + 'static>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
    // Enumerate shareable content. TCC denial surfaces here as a
    // NoShareableContent error from the Swift side.
    let content = SCShareableContent::get().map_err(|e| {
        if is_authorization_error(&e) {
            "not-authorized".to_string()
        } else {
            format!("shareable-content: {e}")
        }
    })?;

    let displays = content.displays();
    let display = displays
        .first()
        .ok_or_else(|| "no-display-available".to_string())?;

    // "Capture everything on this display" — audio follows the display target.
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // We're audio-only from our perspective: ScreenCaptureKit still requires a
    // configured video surface, but since we never register a Screen output
    // handler the video frames are discarded by the stream layer.
    // We're audio-only from our perspective: ScreenCaptureKit still requires a
    // configured video surface, but since we never register a Screen output
    // handler the video frames are discarded by the stream layer.
    //
    // `with_captures_microphone(true)` enables the OS-managed mic tap (macOS
    // 15+), delivered as a separate `SCStreamOutputType::Microphone` output.
    // Unlike a raw cpal HAL client, this coexists with a VoIP app (WhatsApp,
    // Zoom, …) that has the mic in voice-processing mode — it's the same path
    // the system screen recorder uses to capture your voice during a call.
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_captures_microphone(mic_wav_path.is_some())
        .with_sample_rate(SAMPLE_RATE as i32)
        .with_channel_count(i32::from(CHANNELS))
        .with_excludes_current_process_audio(true);

    // Create the file now; defer WavWriter creation until the first sample
    // buffer so we can read the actual delivered sample rate and channel count
    // from its format description.
    let file = File::create(wav_path).map_err(|e| format!("create-wav: {e}"))?;
    let writer = Arc::new(Mutex::new(PendingWriter {
        file: Some(BufWriter::new(file)),
        writer: None,
        init_failed: false,
    }));

    let output = AudioOutput {
        paused: paused.clone(),
        writer: Arc::clone(&writer),
        amplitude: Some(amplitude),
        accept: SCStreamOutputType::Audio,
        label: "system",
        logged: AtomicBool::new(false),
    };

    // Delegate that reports an unexpected mid-session stream death (display
    // sleep, screen lock, replayd restart, …). Without this the stream dies
    // silently and we keep "recording" nothing until the user hits stop.
    let teardown = Arc::new(AtomicBool::new(false));
    let notifier = Arc::new(StopNotifier {
        teardown: Arc::clone(&teardown),
        notified: AtomicBool::new(false),
        callback: on_stopped,
    });
    let delegate = {
        let n_err = Arc::clone(&notifier);
        let n_stop = Arc::clone(&notifier);
        StreamCallbacks::new()
            .on_error(move |e| n_err.notify(Some(e.to_string())))
            .on_stop(move |err| n_stop.notify(err))
    };

    let mut stream = SCStream::new_with_delegate(&filter, &config, delegate);
    if stream
        .add_output_handler(output, SCStreamOutputType::Audio)
        .is_none()
    {
        return Err("add-audio-handler-failed".to_string());
    }

    // SCStream-native microphone tap, when requested. Separate WAV + writer,
    // same lazy-format / planar-aware handler as the system tap.
    let mic_writer = if let Some(mic_path) = mic_wav_path {
        let mic_file = File::create(mic_path).map_err(|e| format!("create-mic-wav: {e}"))?;
        let mw = Arc::new(Mutex::new(PendingWriter {
            file: Some(BufWriter::new(mic_file)),
            writer: None,
            init_failed: false,
        }));
        let mic_output = AudioOutput {
            paused: paused.clone(),
            writer: Arc::clone(&mw),
            amplitude: None,
            accept: SCStreamOutputType::Microphone,
            label: "sck-mic",
            logged: AtomicBool::new(false),
        };
        if stream
            .add_output_handler(mic_output, SCStreamOutputType::Microphone)
            .is_none()
        {
            return Err("add-mic-handler-failed".to_string());
        }
        Some(mw)
    } else {
        None
    };

    if let Err(e) = stream.start_capture() {
        return Err(if is_authorization_error(&e) {
            "not-authorized".to_string()
        } else {
            format!("start-capture: {e}")
        });
    }

    Ok(Box::new(SystemAudioHandle {
        _stream: stream,
        writer,
        mic_writer,
        teardown,
    }))
}

/// TCC / permission denial detection. ScreenCaptureKit surfaces denial in a
/// couple of forms depending on whether the user has never been prompted or
/// has explicitly declined.
fn is_authorization_error(e: &SCError) -> bool {
    match e {
        SCError::PermissionDenied(_) => true,
        SCError::NoShareableContent(_) => true,
        SCError::SCStreamError { code, .. } => matches!(
            code,
            SCStreamErrorCode::UserDeclined | SCStreamErrorCode::MissingEntitlements
        ),
        // CaptureStartFailed / StreamError come through as strings; try to sniff
        // well-known codes embedded by the crate's from_raw mapping.
        SCError::CaptureStartFailed(msg) | SCError::StreamError(msg) => {
            msg.contains("UserDeclined") || msg.contains("MissingEntitlements")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_spec_is_48k_stereo_s16() {
        let s = wav_spec();
        assert_eq!(s.sample_rate, 48_000);
        assert_eq!(s.channels, 2);
        assert_eq!(s.bits_per_sample, 16);
        assert_eq!(s.sample_format, hound::SampleFormat::Int);
    }

    use std::sync::atomic::AtomicUsize;

    fn notifier_with_counter(teardown: bool) -> (Arc<StopNotifier>, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let count_cb = Arc::clone(&count);
        let n = Arc::new(StopNotifier {
            teardown: Arc::new(AtomicBool::new(teardown)),
            notified: AtomicBool::new(false),
            callback: Box::new(move |_| {
                count_cb.fetch_add(1, Ordering::Relaxed);
            }),
        });
        (n, count)
    }

    #[test]
    fn stop_notifier_fires_once_across_both_delegate_paths() {
        let (n, count) = notifier_with_counter(false);
        n.notify(Some("display sleep".into())); // did_stop_with_error
        n.notify(None); // stream_did_stop for the same event
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stop_notifier_is_silent_during_teardown() {
        let (n, count) = notifier_with_counter(true);
        n.notify(None);
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }
}
