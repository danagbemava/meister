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

/// Opaque handle returned to the caller. `Drop` finalizes the WAV writer and
/// releases the SCStream. We box this as `dyn Any + Send` in the public API
/// so the registry stays agnostic across capture backends.
struct SystemAudioHandle {
    // SCStream's Drop stops the capture via sc_stream_release. We keep it
    // alive in the handle so the stream runs for the session's lifetime.
    _stream: SCStream,
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        // Finalize the WAV first so the header is written while any in-flight
        // callback that still held the mutex has a chance to drain. The
        // stream release in SCStream::drop happens after this scope ends.
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(w) = guard.take() {
                let _ = w.finalize();
            }
        }
    }
}

/// Output handler that drops frames while paused, otherwise converts
/// interleaved f32 to i16 and writes to the shared `WavWriter`.
struct AudioOutput {
    paused: Arc<AtomicBool>,
    writer: Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>,
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

        // ScreenCaptureKit delivers interleaved f32 by default when captures_audio
        // is set. Sum the bytes across any buffers present (normally one).
        let mut guard = match self.writer.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let Some(writer) = guard.as_mut() else {
            return;
        };

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
    }
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

    // Create the WAV writer. hound infers stereo-interleaved layout from the
    // spec and writes samples in the order we feed them.
    let file = File::create(wav_path).map_err(|e| format!("create-wav: {e}"))?;
    let writer = WavWriter::new(BufWriter::new(file), wav_spec())
        .map_err(|e| format!("wav-writer-init: {e}"))?;
    let writer = Arc::new(Mutex::new(Some(writer)));

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
