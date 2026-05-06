//! cpal-backed mic capture. Writes interleaved i16 PCM to a WAV file at the
//! same sample rate and channel count as `system_audio`. Resamples/channel-
//! fixes on the fly if the mic's native format differs.

use super::system_audio::{CHANNELS, SAMPLE_RATE, wav_spec};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::WavWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn start_mic_capture(
    wav_path: &Path,
    paused: Arc<AtomicBool>,
    amplitude: Arc<super::amplitude::AmplitudeEmitter>,
) -> Result<Box<dyn std::any::Any + Send>, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;

    let dev_channels = supported.channels();
    let dev_rate = supported.sample_rate();
    let dev_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let writer = WavWriter::create(wav_path, wav_spec())
        .map_err(|e| format!("create mic wav: {e}"))?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let paused_cb = paused.clone();
    let writer_cb = writer.clone();
    let amplitude_f32 = amplitude.clone();
    let amplitude_i16 = amplitude.clone();
    let amplitude_u16 = amplitude.clone();

    let err_fn = |err| eprintln!("mic cpal stream error: {err}");

    let stream = match dev_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                amplitude_f32.push(super::amplitude::compute_rms(data));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, data, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                amplitude_i16.push(super::amplitude::compute_rms(&f32s));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, &f32s, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                if paused_cb.load(Ordering::Relaxed) { return; }
                let i16s: Vec<i16> = data.iter().map(|&u| (u as i32 - 32_768) as i16).collect();
                let f32s: Vec<f32> = i16s.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                amplitude_u16.push(super::amplitude::compute_rms(&f32s));
                let mut guard = writer_cb.lock().unwrap();
                let Some(w) = guard.as_mut() else { return; };
                write_resampled_f32(w, &f32s, dev_channels, dev_rate);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported mic sample format: {other:?}")),
    }
    .map_err(|e| format!("build mic input stream: {e}"))?;

    stream.play().map_err(|e| format!("start mic stream: {e}"))?;

    struct Handle {
        _stream: cpal::Stream,
        writer: Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            let mut guard = self.writer.lock().unwrap();
            if let Some(w) = guard.take() {
                let _ = w.finalize();
            }
        }
    }
    Ok(Box::new(Handle { _stream: stream, writer }))
}

fn write_resampled_f32<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    data: &[f32],
    src_channels: u16,
    src_rate: u32,
) {
    let mono: Vec<f32> = if src_channels == 1 {
        data.to_vec()
    } else {
        data.chunks_exact(src_channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / src_channels as f32)
            .collect()
    };
    let resampled: Vec<f32> = if src_rate == SAMPLE_RATE {
        mono
    } else {
        linear_resample(&mono, src_rate, SAMPLE_RATE)
    };
    for s in resampled {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * i16::MAX as f32) as i16;
        for _ in 0..CHANNELS {
            let _ = writer.write_sample(i);
        }
    }
}

fn linear_resample(src: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src.is_empty() { return Vec::new(); }
    let ratio = dst_rate as f64 / src_rate as f64;
    let dst_len = (src.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(dst_len);
    for i in 0..dst_len {
        let src_idx_f = i as f64 / ratio;
        let idx0 = src_idx_f.floor() as usize;
        let idx1 = (idx0 + 1).min(src.len() - 1);
        let frac = (src_idx_f - idx0 as f64) as f32;
        let s = src[idx0] * (1.0 - frac) + src[idx1] * frac;
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_resample_double_rate_doubles_length() {
        let src: Vec<f32> = (0..100).map(|i| (i as f32) / 100.0).collect();
        let out = linear_resample(&src, 24_000, 48_000);
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn linear_resample_passthrough_at_same_rate() {
        let src: Vec<f32> = vec![0.1, 0.2, 0.3];
        let out = linear_resample(&src, 48_000, 48_000);
        assert_eq!(out, src);
    }
}
