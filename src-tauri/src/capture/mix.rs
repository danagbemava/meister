//! ffmpeg-based mix + encode. Produces the final WebM/Opus at a caller-chosen
//! output path. When only system audio was captured, re-encodes to opus; when
//! both are present, runs amix.

use std::path::Path;
use std::process::Command;

pub fn encode_final(
    system_wav: &Path,
    mic_wav: Option<&Path>,
    output_webm: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_webm.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create output dir: {e}"))?;
    }
    let args: Vec<String> = if let Some(mic) = mic_wav {
        // Per-input aresample to 48 kHz *before* amix is required: amix mixes
        // its inputs at the sample rate of input #0 and only auto-aligns when
        // both inputs already declare the same rate. The system_audio WAV is
        // now written at the platform's actual delivered rate (which can be
        // 24/16/12 kHz depending on hardware state), so explicitly upsampling
        // each input first guarantees correct alignment.
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-i".into(), mic.to_string_lossy().into_owned(),
            "-filter_complex".into(),
            "[0:a]aresample=48000[s0];\
             [1:a]aresample=48000[s1];\
             [s0][s1]amix=inputs=2:duration=longest:dropout_transition=0[out]".into(),
            "-map".into(), "[out]".into(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    } else {
        // Single-input path: aresample to 48 kHz so the opus encoder always
        // sees its standard rate even if the system_audio WAV is at a lower
        // delivered rate.
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-af".into(), "aresample=48000".into(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    };

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!("ffmpeg error:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_sine(path: &Path, secs: u32, freq: f32) {
        write_sine_at_rate(path, secs, freq, 48_000, 2);
    }

    fn write_sine_at_rate(path: &Path, secs: u32, freq: f32, rate: u32, channels: u16) {
        let spec = WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for i in 0..rate * secs {
            let t = i as f32 / rate as f32;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin();
            let v = (s * 16_000.0) as i16;
            for _ in 0..channels {
                w.write_sample(v).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn encode_single_source_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        write_sine(&sys, 1, 440.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, None, &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0, "output webm should be non-empty");
    }

    #[test]
    fn encode_two_sources_amix_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        let mic = dir.path().join("mic.wav");
        write_sine(&sys, 1, 440.0);
        write_sine(&mic, 1, 880.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, Some(&mic), &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0);
    }

    #[test]
    fn encode_two_sources_with_mismatched_input_rates() {
        // Regression for the "chipmunk remote speakers" bug: SCStream-delivered
        // system.wav can be at a non-48 kHz rate (24 kHz, 16 kHz, …) while mic.wav
        // is always 48 kHz. The per-input aresample in encode_final must align
        // both to 48 kHz before amix, otherwise the lower-rate input plays back
        // sped-up and pitch-shifted.
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        let mic = dir.path().join("mic.wav");
        write_sine_at_rate(&sys, 1, 440.0, 24_000, 2);
        write_sine_at_rate(&mic, 1, 880.0, 48_000, 2);
        let out = dir.path().join("out.webm");
        encode_final(&sys, Some(&mic), &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0);
    }
}
