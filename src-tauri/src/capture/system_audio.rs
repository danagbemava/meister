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
use std::sync::{Arc, Mutex, OnceLock};

use hound::WavWriter;
use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::error::{SCError, SCStreamErrorCode};
use screencapturekit::prelude::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutputTrait,
    SCStreamOutputType,
};

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

/// Opaque handle returned to the caller. `Drop` finalizes the WAV writer and
/// releases the SCStream. We box this as `dyn Any + Send` in the public API
/// so the registry stays agnostic across capture backends.
struct SystemAudioHandle {
    // SCStream's Drop stops the capture via sc_stream_release. We keep it
    // alive in the handle so the stream runs for the session's lifetime.
    _stream: SCStream,
    writer: Arc<Mutex<PendingWriter>>,
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        // Finalize the WAV first so the header is written while any in-flight
        // callback that still held the mutex has a chance to drain. The
        // stream release in SCStream::drop happens after this scope ends.
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(w) = guard.writer.take() {
                let _ = w.finalize();
            }
            // The BufWriter<File> drops here if the writer was never created
            // (no samples received). File handle closes normally.
        }
    }
}

/// Output handler that drops frames while paused, otherwise reads the actual
/// delivered audio format on first sample, writes samples to the lazily-created
/// WAV, and handles both interleaved and non-interleaved (planar) buffer layouts.
struct AudioOutput {
    paused: Arc<AtomicBool>,
    writer: Arc<Mutex<PendingWriter>>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
}

impl SCStreamOutputTrait for AudioOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        // We only registered for Audio, but defensively ignore anything else.
        if of_type != SCStreamOutputType::Audio {
            return;
        }
        if self.paused.load(Ordering::Relaxed) {
            return;
        }

        let Some(list) = sample_buffer.audio_buffer_list() else {
            return;
        };

        // Diagnostic: log the actual format of the first buffer received,
        // exactly once per process. The `with_sample_rate()` and
        // `with_channel_count()` we passed to SCStreamConfiguration are
        // requests, not guarantees — this surfaces what the platform
        // actually delivered, which is what determines correct WAV-header
        // sample rate. Keep this until the capture pipeline reads the
        // delivered rate instead of trusting the request.
        static LOGGED: OnceLock<()> = OnceLock::new();
        LOGGED.get_or_init(|| {
            let bufs: Vec<_> = list.iter().collect();
            let first_bytes = bufs.first().map(|b| b.data().len()).unwrap_or(0);
            let n_samples = sample_buffer.num_samples();
            if let Some(fd) = sample_buffer.format_description() {
                eprintln!(
                    "[capture/system_audio] FIRST BUFFER: requested {} Hz × {} ch; \
                     delivered rate={:?} Hz, channels={:?}, bits={:?}, \
                     bytes_per_frame={:?}, is_float={}, format_flags={:?}; \
                     ABL buffers={}, first_buffer_bytes={}, num_samples={}",
                    SAMPLE_RATE, CHANNELS,
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
                    "[capture/system_audio] FIRST BUFFER: no format_description; \
                     requested {} Hz × {} ch; ABL buffers={}, first_buffer_bytes={}, \
                     num_samples={}",
                    SAMPLE_RATE, CHANNELS, bufs.len(), first_bytes, n_samples,
                );
            }
        });

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

        self.amplitude.push(super::amplitude::compute_rms(&frame));
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
pub fn start_system_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
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
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
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
        paused,
        writer: Arc::clone(&writer),
        amplitude,
    };

    let mut stream = SCStream::new(&filter, &config);
    if stream
        .add_output_handler(output, SCStreamOutputType::Audio)
        .is_none()
    {
        return Err("add-audio-handler-failed".to_string());
    }

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
}
