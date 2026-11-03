//! SCStream wiring for macOS system audio capture.
//!
//! Produces a 48 kHz / stereo / 16-bit signed PCM WAV file for the duration
//! the returned handle is kept alive. Dropping the handle stops the stream
//! and finalizes the WAV.
//!
//! Pause/Resume is implemented by toggling the `paused` flag the caller owns.
//! While paused, audio callbacks drop incoming frames, and the paused span is
//! left out of the timeline.
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
//! Audio arrives as `CMSampleBuffer`s whose format (rate, channels, float or
//! int16, interleaved or planar) can differ per buffer. Each buffer is
//! converted to 48 kHz stereo i16 and placed by its presentation timestamp.

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

type Writer = WavWriter<BufWriter<File>>;

/// Frames of timeline error tolerated before we pad with silence or trim.
const DRIFT_TOLERANCE_FRAMES: u64 = SAMPLE_RATE as u64 / 50;

/// Shared capture clock for every tap on one stream. Maps a buffer's
/// presentation timestamp to seconds of un-paused recording time, so gaps in
/// delivery become silence and both tracks stay on one timeline.
#[derive(Default)]
struct Timeline {
    t0: Option<f64>,
    paused_total: f64,
    paused_since: Option<f64>,
}

impl Timeline {
    fn position(&mut self, pts: f64, paused: bool) -> Option<f64> {
        if paused {
            self.paused_since.get_or_insert(pts);
            return None;
        }
        if let Some(since) = self.paused_since.take() {
            if self.t0.is_some() {
                self.paused_total += pts - since;
            }
        }
        let t0 = *self.t0.get_or_insert(pts);
        Some((pts - t0 - self.paused_total).max(0.0))
    }
}

struct Track {
    writer: Option<Writer>,
    frames_written: u64,
    format: Option<(u32, u32)>,
}

impl Track {
    fn new(path: &Path) -> Result<Arc<Mutex<Self>>, String> {
        let writer = WavWriter::create(path, wav_spec()).map_err(|e| format!("create-wav: {e}"))?;
        Ok(Arc::new(Mutex::new(Self { writer: Some(writer), frames_written: 0, format: None })))
    }

    /// Write 48 kHz stereo frames so the first one lands at `position` seconds.
    fn write_at(&mut self, position: f64, frames: &[f32]) {
        let Some(w) = self.writer.as_mut() else { return };
        let expected = (position * f64::from(SAMPLE_RATE)) as u64;
        let mut frames = frames;
        if expected > self.frames_written + DRIFT_TOLERANCE_FRAMES {
            for _ in 0..(expected - self.frames_written) * u64::from(CHANNELS) {
                let _ = w.write_sample(0i16);
            }
            self.frames_written = expected;
        } else if self.frames_written > expected + DRIFT_TOLERANCE_FRAMES {
            let skip = ((self.frames_written - expected) as usize * CHANNELS as usize).min(frames.len());
            frames = &frames[skip..];
        }
        for &s in frames {
            let _ = w.write_sample((s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
        }
        self.frames_written += (frames.len() / CHANNELS as usize) as u64;
    }
}

struct SystemAudioHandle {
    stream: SCStream,
    tracks: Vec<Arc<Mutex<Track>>>,
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
        for track in &self.tracks {
            if let Ok(mut t) = track.lock() {
                if let Some(w) = t.writer.take() {
                    if let Err(e) = w.finalize() {
                        eprintln!("[capture/system_audio] finalize WAV failed: {e}");
                    }
                }
            }
        }
    }
}

/// One handler per tap (system audio, and the ScreenCaptureKit mic when
/// enabled). Converts every buffer from its own delivered format to the fixed
/// 48 kHz stereo WAV format, because the delivered rate can change mid-stream
/// (for example when Bluetooth headphones switch to call mode).
struct AudioOutput {
    paused: Arc<AtomicBool>,
    timeline: Arc<Mutex<Timeline>>,
    track: Arc<Mutex<Track>>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
    label: &'static str,
}

impl SCStreamOutputTrait for AudioOutput {
    fn did_output_sample_buffer(&self, sample_buffer: CMSampleBuffer, _of_type: SCStreamOutputType) {
        let Some(pts) = sample_buffer.presentation_timestamp().as_seconds() else { return };
        let paused = self.paused.load(Ordering::Relaxed);
        let Some(position) = self.timeline.lock().ok().and_then(|mut t| t.position(pts, paused)) else {
            return;
        };
        let Some(fd) = sample_buffer.format_description() else { return };
        let (Some(rate), Some(channels)) = (fd.audio_sample_rate(), fd.audio_channel_count()) else {
            return;
        };
        let Some(list) = sample_buffer.audio_buffer_list() else { return };
        let bufs: Vec<&[u8]> = list.iter().map(|b| b.data()).collect();
        let bits = fd.audio_bits_per_channel().unwrap_or(32);
        let Some(interleaved) = decode(&bufs, fd.audio_is_float(), bits) else { return };
        let frames = to_48k_stereo(&interleaved, channels as usize, rate as u32);

        self.amplitude.push(super::amplitude::compute_rms(&frames));
        let Ok(mut track) = self.track.lock() else { return };
        let format = (rate as u32, channels);
        if track.format != Some(format) {
            eprintln!(
                "[capture/system_audio] [{}] delivered format: {} Hz × {} ch, {} bit{}",
                self.label, format.0, format.1, bits, if fd.audio_is_float() { " float" } else { "" },
            );
            track.format = Some(format);
        }
        track.write_at(position, &frames);
    }
}

/// Decode raw buffers into interleaved f32 samples. More than one buffer means
/// planar layout: one buffer per channel.
fn decode(bufs: &[&[u8]], is_float: bool, bits: u32) -> Option<Vec<f32>> {
    let read: fn(&[u8]) -> f32 = match (is_float, bits) {
        (true, 32) => |b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]),
        (false, 16) => |b| f32::from(i16::from_ne_bytes([b[0], b[1]])) / f32::from(i16::MAX),
        _ => return None,
    };
    let width = bits as usize / 8;
    if bufs.len() <= 1 {
        let bytes = bufs.first().copied().unwrap_or(&[]);
        return Some(bytes.chunks_exact(width).map(read).collect());
    }
    let per_channel = bufs.iter().map(|b| b.len() / width).min().unwrap_or(0);
    let mut out = Vec::with_capacity(per_channel * bufs.len());
    for i in 0..per_channel {
        for ch in bufs {
            out.push(read(&ch[i * width..]));
        }
    }
    Some(out)
}

/// Convert interleaved samples at any rate and channel count to 48 kHz
/// interleaved stereo. Mono is duplicated; extra channels are dropped.
fn to_48k_stereo(interleaved: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    if channels == 0 || rate == 0 {
        return Vec::new();
    }
    let frames = interleaved.len() / channels;
    let src = |i: usize, c: usize| interleaved[i * channels + c.min(channels - 1)];
    let out_frames = (frames as u64 * u64::from(SAMPLE_RATE) / u64::from(rate)) as usize;
    let step = f64::from(rate) / f64::from(SAMPLE_RATE);
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let pos = i as f64 * step;
        let i0 = pos as usize;
        let i1 = (i0 + 1).min(frames - 1);
        let frac = (pos - i0 as f64) as f32;
        for c in 0..2 {
            out.push(src(i0, c) * (1.0 - frac) + src(i1, c) * frac);
        }
    }
    out
}

/// Start a background SCStream that mixes every process's audio into a single
/// stereo 48 kHz i16 WAV file. Returns a handle whose `Drop` tears everything
/// down and finalizes the file. Returns `Err("not-authorized")` when Screen
/// Recording permission has not been granted.
pub fn start_system_capture(
    wav_path: &Path,
    mic_wav_path: Option<&Path>,
    paused: Arc<AtomicBool>,
    system_meter: Arc<super::amplitude::AmplitudeEmitter>,
    mic_meter: Arc<super::amplitude::AmplitudeEmitter>,
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

    // Audio-only for us: ScreenCaptureKit still needs a tiny video surface, but
    // with no Screen output handler the video frames are discarded.
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_captures_microphone(mic_wav_path.is_some())
        .with_sample_rate(SAMPLE_RATE as i32)
        .with_channel_count(i32::from(CHANNELS))
        .with_excludes_current_process_audio(true);

    let timeline = Arc::new(Mutex::new(Timeline::default()));
    let mut stream = SCStream::new(&filter, &config);
    let mut tracks = Vec::new();
    let taps = [
        (Some(wav_path), SCStreamOutputType::Audio, system_meter, "system"),
        (mic_wav_path, SCStreamOutputType::Microphone, mic_meter, "sck-mic"),
    ];
    for (path, kind, amplitude, label) in taps {
        let Some(path) = path else { continue };
        let track = Track::new(path)?;
        let output = AudioOutput {
            paused: paused.clone(),
            timeline: timeline.clone(),
            track: track.clone(),
            amplitude,
            label,
        };
        if stream.add_output_handler(output, kind).is_none() {
            return Err(format!("add-{label}-handler-failed"));
        }
        tracks.push(track);
    }

    if let Err(e) = stream.start_capture() {
        return Err(if is_authorization_error(&e) {
            "not-authorized".to_string()
        } else {
            format!("start-capture: {e}")
        });
    }

    Ok(Box::new(SystemAudioHandle { stream, tracks }))
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

    #[test]
    fn timeline_excludes_paused_time() {
        let mut t = Timeline::default();
        assert_eq!(t.position(100.0, false), Some(0.0));
        assert_eq!(t.position(102.0, false), Some(2.0));
        assert_eq!(t.position(103.0, true), None);
        assert_eq!(t.position(108.0, false), Some(3.0));
    }

    #[test]
    fn timeline_ignores_pause_before_first_buffer() {
        let mut t = Timeline::default();
        assert_eq!(t.position(50.0, true), None);
        assert_eq!(t.position(55.0, false), Some(0.0));
        assert_eq!(t.position(56.0, false), Some(1.0));
    }

    #[test]
    fn mono_24k_becomes_48k_stereo_at_same_duration() {
        let out = to_48k_stereo(&[0.5; 240], 1, 24_000);
        assert_eq!(out.len(), 480 * 2);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn planar_int16_is_interleaved() {
        let left: Vec<u8> = [1000i16, 2000].iter().flat_map(|s| s.to_ne_bytes()).collect();
        let right: Vec<u8> = [-1000i16, -2000].iter().flat_map(|s| s.to_ne_bytes()).collect();
        let out = decode(&[&left, &right], false, 16).unwrap();
        let ints: Vec<i16> = out.iter().map(|s| (s * f32::from(i16::MAX)).round() as i16).collect();
        assert_eq!(ints, vec![1000, -1000, 2000, -2000]);
    }

    #[test]
    fn gap_in_delivery_is_filled_with_silence() {
        let dir = std::env::temp_dir().join(format!("meister-gap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gap.wav");
        let track = Track::new(&path).unwrap();
        {
            let mut t = track.lock().unwrap();
            t.write_at(0.0, &vec![0.5; 4800 * 2]);
            t.write_at(1.0, &vec![0.5; 4800 * 2]);
            t.writer.take().unwrap().finalize().unwrap();
        }
        let r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.duration(), 48_000 + 4800);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
